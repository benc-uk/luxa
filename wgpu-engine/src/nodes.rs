use crate::{
  engine::{MeshHandle, Node3DHandle},
  helpers::uniform_entry,
};
use glam::{Mat4, Quat, Vec3};
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct NodeUniform {
  model_matrix: [f32; 16],
  normal_matrix: [f32; 16],
}

pub struct Node3D {
  kind: NodeKind,

  position: Vec3,
  rotation: Quat,
  scale: Vec3,
  local_matrix: Mat4,
  world_matrix: Mat4,
  parent: Option<Node3DHandle>,
  children: Option<Vec<Node3DHandle>>,

  // GPU resources
  bind_group: wgpu::BindGroup,
  uniform: NodeUniform,
  uniform_buffer: wgpu::Buffer, // Holds the world transform matrix for this node, which is updated each frame
}

pub enum NodeKind {
  Empty,
  Mesh(MeshData),
  Camera(CameraData),
  Light(LightData),
}

pub struct CameraData {
  pub fovy: f32,
  pub znear: f32,
  pub zfar: f32,
  pub target: Vec3, // world space
  pub up: Vec3,     // usually Vec3::Y
}

pub struct MeshData {
  pub mesh_handles: Vec<MeshHandle>,
}

pub struct LightData {
  pub color: Vec3,
  pub intensity: f32,
}

impl CameraData {
  pub fn view_proj(&self, world_pos: Vec3, aspect: f32) -> Mat4 {
    let eye = if (self.target - world_pos).length_squared() < 1e-12 {
      world_pos + Vec3::NEG_Z // degenerate guard: target == eye
    } else {
      world_pos
    };
    let view = glam::camera::rh::view::look_at_mat4(eye, self.target, self.up);
    let proj = glam::camera::rh::proj::directx::perspective(self.fovy.to_radians(), aspect, self.znear, self.zfar);
    proj * view
  }
}

impl Node3D {
  pub(crate) fn new(device: &wgpu::Device, position: Vec3, rotation: Quat, scale: Vec3) -> Self {
    let uniform = NodeUniform {
      model_matrix: Mat4::IDENTITY.to_cols_array(), // Updated below
      normal_matrix: Mat4::IDENTITY.to_cols_array(),
    };

    let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
      label: Some("Node Uniform Buffer"),
      contents: bytemuck::cast_slice(&[uniform]),
      usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    });

    let bind_group_layout = Self::get_bind_group_layout(device);

    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
      label: Some("Node Bind Group"),
      layout: &bind_group_layout,
      entries: &[wgpu::BindGroupEntry {
        binding: 0,
        resource: uniform_buffer.as_entire_binding(),
      }],
    });

    let mut node = Node3D {
      kind: NodeKind::Empty,
      position,
      rotation,
      scale,
      local_matrix: Mat4::IDENTITY, // Also updated below
      world_matrix: Mat4::IDENTITY,
      parent: None,
      children: None,

      bind_group,
      uniform,
      uniform_buffer,
    };

    node.update();
    node
  }

  pub(crate) fn new_mesh(device: &wgpu::Device, meshes: Vec<MeshHandle>, position: Vec3, rotation: Quat, scale: Vec3) -> Self {
    let mut node = Self::new(device, position, rotation, scale);
    node.kind = NodeKind::Mesh(MeshData { mesh_handles: meshes });
    node
  }

  pub(crate) fn new_camera(device: &wgpu::Device, position: Vec3, look_at: Vec3, scale: Vec3, fovy: f32, znear: f32, zfar: f32) -> Self {
    let rotation = Quat::from_rotation_arc(glam::vec3(0.0, 0.0, -1.0), (look_at - position).normalize());
    let mut node = Self::new(device, position, rotation, scale);
    node.kind = NodeKind::Camera(CameraData {
      fovy,
      znear,
      zfar,
      target: look_at,
      up: Vec3::Y,
    });

    node
  }

  pub(crate) fn new_light(device: &wgpu::Device, position: Vec3, color: Vec3, intensity: f32) -> Self {
    let rotation = Quat::IDENTITY;
    let scale = Vec3::ONE;
    let mut node = Self::new(device, position, rotation, scale);
    node.kind = NodeKind::Light(LightData { color, intensity });
    node
  }

  pub(crate) fn set_parent(&mut self, parent: Node3DHandle) {
    self.parent = Some(parent);
  }

  pub(crate) fn add_child(&mut self, child: Node3DHandle) {
    if let Some(children) = &mut self.children {
      children.push(child);
    } else {
      self.children = Some(vec![child]);
    }
  }

  pub(crate) fn children(&self) -> &[Node3DHandle] {
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
    self.update();
  }

  pub fn set_world_matrix(&mut self, world_matrix: Mat4) {
    self.world_matrix = world_matrix;
  }

  pub fn look_at(&mut self, target: Vec3) {
    let direction = (target - self.position).normalize();
    self.rotation = Quat::from_rotation_arc(glam::vec3(0.0, 0.0, -1.0), direction);
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

  pub(crate) fn get_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
      label: Some("Node Bind Group Layout"),
      entries: &[uniform_entry(0, wgpu::ShaderStages::VERTEX)],
    })
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

  pub(crate) fn view_proj(&self, aspect: f32) -> Option<Mat4> {
    match &self.kind {
      NodeKind::Camera(data) => Some(data.view_proj(self.world_position(), aspect)),
      _ => None,
    }
  }

  pub(crate) fn is_light(&self) -> bool {
    matches!(self.kind, NodeKind::Light(_))
  }

  pub(crate) fn light_data(&self) -> Option<&LightData> {
    match &self.kind {
      NodeKind::Light(data) => Some(data),
      _ => None,
    }
  }
}
