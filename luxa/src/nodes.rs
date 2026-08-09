mod camera;

use crate::common::Aabb;
use crate::engine::{MeshHandle, NodeHandle};
use crate::models::Mesh;
use glam::{Mat4, Quat, Vec3};
use slotmap::SlotMap;
use wgpu::util::DeviceExt;

pub use camera::{CameraDescriptor, CameraOrientation};

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct NodeUniform {
  model_matrix: [f32; 16],
  normal_matrix: [f32; 16],
}

pub struct Node {
  kind: NodeKind,

  position: Vec3,
  rotation: Quat,
  scale: Vec3,
  local_matrix: Mat4,
  world_matrix: Mat4,
  parent: Option<NodeHandle>,
  children: Option<Vec<NodeHandle>>,

  // Size and center of the node's AABB in world space, if it has a mesh or other size/volume
  aabb: Option<Aabb>,
  center: Option<Vec3>,

  // GPU resources
  bind_group: wgpu::BindGroup,
  uniform: NodeUniform,
  uniform_buffer: wgpu::Buffer, // Holds the world transform matrix for this node, which is updated each frame
}

pub(crate) enum NodeKind {
  Empty,
  Mesh(MeshData),
  Camera(camera::CameraData),
  Light(LightData),
}

pub(crate) struct MeshData {
  pub mesh_handles: Vec<MeshHandle>,
}

pub(crate) struct LightData {
  pub color: Vec3,
  pub intensity: f32,
}

impl Node {
  pub(crate) fn new(device: &wgpu::Device, bind_group_layout: &wgpu::BindGroupLayout, position: Vec3, rotation: Quat, scale: Vec3) -> Self {
    let uniform = NodeUniform {
      model_matrix: Mat4::IDENTITY.to_cols_array(), // Updated below
      normal_matrix: Mat4::IDENTITY.to_cols_array(),
    };

    let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
      label: Some("Node Uniform Buffer"),
      contents: bytemuck::cast_slice(&[uniform]),
      usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    });

    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
      label: Some("Node Bind Group"),
      layout: &bind_group_layout,
      entries: &[wgpu::BindGroupEntry {
        binding: 0,
        resource: uniform_buffer.as_entire_binding(),
      }],
    });

    let mut node = Node {
      kind: NodeKind::Empty,
      position,
      rotation,
      scale,
      local_matrix: Mat4::IDENTITY, // Also updated below
      world_matrix: Mat4::IDENTITY,
      parent: None,
      children: None,
      aabb: None,
      center: None,

      bind_group,
      uniform,
      uniform_buffer,
    };

    node.update();
    node
  }

  pub(crate) fn new_mesh(
    device: &wgpu::Device,
    bind_group_layout: &wgpu::BindGroupLayout,
    mesh_arena: &SlotMap<MeshHandle, Mesh>,
    mesh_handles: Vec<MeshHandle>,
    position: Vec3,
    rotation: Quat,
    scale: Vec3,
  ) -> Self {
    let mut node = Self::new(device, bind_group_layout, position, rotation, scale);

    // Compute the AABB and center of the node based on its meshes. This is useful for culling, camera framing, etc.
    // The handles are just keys, so we look each mesh up in the engine's mesh arena to read its local AABB.
    let mut aabb: Option<Aabb> = None;
    for handle in &mesh_handles {
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

    node.kind = NodeKind::Mesh(MeshData { mesh_handles });
    node
  }

  pub(crate) fn new_light(device: &wgpu::Device, bind_group_layout: &wgpu::BindGroupLayout, position: Vec3, color: Vec3, intensity: f32) -> Self {
    let rotation = Quat::IDENTITY;
    let scale = Vec3::ONE;
    let mut node = Self::new(device, bind_group_layout, position, rotation, scale);
    node.kind = NodeKind::Light(LightData { color, intensity });
    node
  }

  pub(crate) fn set_parent(&mut self, parent: NodeHandle) {
    self.parent = Some(parent);
  }

  pub(crate) fn parent(&self) -> Option<NodeHandle> {
    self.parent
  }

  pub(crate) fn add_child(&mut self, child: NodeHandle) {
    if let Some(children) = &mut self.children {
      children.push(child);
    } else {
      self.children = Some(vec![child]);
    }
  }

  pub(crate) fn remove_child(&mut self, child: NodeHandle) {
    if let Some(children) = &mut self.children
      && let Some(index) = children.iter().position(|&handle| handle == child)
    {
      children.remove(index);
    }
  }

  pub(crate) fn children(&self) -> &[NodeHandle] {
    if let Some(children) = &self.children { children } else { &[] }
  }

  fn update(&mut self) {
    self.local_matrix = Mat4::from_scale_rotation_translation(self.scale, self.rotation, self.position);
  }

  pub fn local_matrix(&self) -> Mat4 {
    self.local_matrix
  }

  pub fn world_matrix(&self) -> Mat4 {
    self.world_matrix
  }

  pub fn world_position(&self) -> Vec3 {
    // Neat way to get the translation from a 4x4 matrix: the last column is the translation vector, and we can truncate it to a Vec3.
    self.world_matrix.w_axis.truncate()
  }

  pub fn set_position(&mut self, position: Vec3) {
    self.position = position;
    self.update();
  }

  pub fn set_rotation(&mut self, rotation: Quat) {
    self.rotation = rotation;

    if let NodeKind::Camera(camera) = &mut self.kind {
      camera.orientation = CameraOrientation::NodeRotation;
    }

    self.update();
  }

  pub fn set_world_matrix(&mut self, world_matrix: Mat4) {
    self.world_matrix = world_matrix;
  }

  pub fn look_at(&mut self, target: Vec3, up: Vec3) {
    // Cameras are a special case
    if let NodeKind::Camera(camera) = &mut self.kind {
      camera.orientation = CameraOrientation::LookAt { target, up };
      return;
    }

    // For non-camera nodes, we can just set the rotation to look at the target point.
    let direction = target - self.position;
    if direction.length_squared() <= 1e-12 {
      return;
    }

    self.rotation = Quat::from_rotation_arc(Vec3::NEG_Z, direction.normalize());
    self.update();
  }

  pub fn set_scale(&mut self, scale: Vec3) {
    self.scale = scale;
    self.update();
  }

  pub fn position(&self) -> Vec3 {
    self.position
  }

  pub fn rotation(&self) -> Quat {
    self.rotation
  }

  pub fn scale(&self) -> Vec3 {
    self.scale
  }

  pub(crate) fn get_bind_group(&self) -> &wgpu::BindGroup {
    &self.bind_group
  }

  pub(crate) fn upload_world_mat(&mut self, queue: &wgpu::Queue) {
    self.uniform.model_matrix = self.world_matrix.to_cols_array();
    let normal_mat = Mat4::from_mat3(glam::Mat3::from_mat4(self.world_matrix).inverse().transpose());
    self.uniform.normal_matrix = normal_mat.to_cols_array();
    queue.write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[self.uniform]));
  }

  pub(crate) fn mesh_handles(&self) -> &[MeshHandle] {
    match &self.kind {
      NodeKind::Mesh(data) => &data.mesh_handles,
      _ => &[], // Empty / Camera
    }
  }

  pub(crate) fn view_proj(&self, world_rotation: Quat, aspect: f32) -> anyhow::Result<Option<Mat4>> {
    match &self.kind {
      NodeKind::Camera(camera) => Ok(Some(camera.view_proj(self.world_position(), world_rotation, aspect)?)),
      _ => Ok(None),
    }
  }

  pub(crate) fn is_light(&self) -> bool {
    matches!(self.kind, NodeKind::Light(_))
  }

  pub(crate) fn is_mesh(&self) -> bool {
    matches!(self.kind, NodeKind::Mesh(_))
  }

  pub(crate) fn light_data(&self) -> Option<&LightData> {
    match &self.kind {
      NodeKind::Light(data) => Some(data),
      _ => None,
    }
  }

  pub fn aabb(&self) -> Option<Aabb> {
    self.aabb
  }

  pub fn center(&self) -> Option<Vec3> {
    self.center
  }
}
