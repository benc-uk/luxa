mod camera;
mod light;
mod mesh;

use crate::common::Aabb;
use crate::engine::{MeshHandle, NodeHandle};
use crate::{SceneHandle, Transform};
use glam::{Mat4, Quat, Vec3};
pub(crate) use light::LightData;
use wgpu::util::DeviceExt;

pub use camera::{CameraDescriptor, CameraHandle, CameraOrientation};
pub use light::{LightDescriptor, LightHandle};
pub(crate) use mesh::MeshData;
pub use mesh::MeshNodeDescriptor;

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct NodeUniform {
  model_matrix: [f32; 16],
  normal_matrix: [f32; 16],
}

pub struct Node {
  kind: NodeKind,
  scene: Option<SceneHandle>,

  transform: Transform,
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

impl Node {
  pub(crate) fn new(device: &wgpu::Device, bind_group_layout: &wgpu::BindGroupLayout, transform: Transform) -> Self {
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
      scene: None,
      transform,
      local_matrix: Mat4::IDENTITY,
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
    self.local_matrix = Mat4::from_scale_rotation_translation(self.transform.scale, self.transform.rotation, self.transform.position);
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

  pub fn transform(&self) -> Transform {
    self.transform
  }

  pub fn set_transform(&mut self, transform: Transform) -> &mut Self {
    self.transform = transform;

    if let NodeKind::Camera(camera) = &mut self.kind {
      camera.orientation = CameraOrientation::NodeRotation;
    }

    self.update();
    self
  }

  pub fn set_position(&mut self, position: Vec3) -> &mut Self {
    self.transform.position = position;
    self.update();
    self
  }

  pub fn set_rotation(&mut self, rotation: Quat) -> &mut Self {
    self.transform.rotation = rotation;

    if let NodeKind::Camera(camera) = &mut self.kind {
      camera.orientation = CameraOrientation::NodeRotation;
    }

    self.update();
    self
  }

  pub fn set_world_matrix(&mut self, world_matrix: Mat4) {
    self.world_matrix = world_matrix;
  }

  pub fn look_at(&mut self, target: Vec3, up: Vec3) -> &mut Self {
    // Cameras are a special case
    if let NodeKind::Camera(camera) = &mut self.kind {
      camera.orientation = CameraOrientation::LookAt { target, up };
      return self;
    }

    // For non-camera nodes, we can just set the rotation to look at the target point.
    let direction = target - self.transform.position;
    if direction.length_squared() <= 1e-12 {
      return self;
    }

    self.transform.rotation = Quat::from_rotation_arc(Vec3::NEG_Z, direction.normalize());
    self.update();
    self
  }

  pub fn set_scale(&mut self, scale: Vec3) -> &mut Self {
    self.transform.scale = scale;
    self.update();
    self
  }

  pub fn position(&self) -> Vec3 {
    self.transform.position
  }

  pub fn rotation(&self) -> Quat {
    self.transform.rotation
  }

  pub fn scale(&self) -> Vec3 {
    self.transform.scale
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
      NodeKind::Mesh(data) => &data.meshes,
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

  pub fn aabb(&self) -> Option<Aabb> {
    self.aabb
  }

  pub fn center(&self) -> Option<Vec3> {
    self.center
  }

  pub(crate) fn scene(&self) -> SceneHandle {
    self.scene.expect("node is always attached to a scene before use")
  }

  pub(crate) fn set_scene(&mut self, scene: SceneHandle) {
    self.scene = Some(scene);
  }
}
