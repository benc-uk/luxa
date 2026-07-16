// ======================================================================================
// WGPU & GLAM based camera implementation. This is very basic
// ======================================================================================

use glam::{Mat4, Vec3};
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct CameraUniform {
  view_proj: [f32; 16],
}

impl CameraUniform {
  fn new() -> Self {
    Self {
      view_proj: Mat4::IDENTITY.to_cols_array(),
    }
  }
}

pub struct Camera {
  pub position: Vec3,
  pub target: Vec3,
  up: Vec3,
  aspect: f32,
  fovy: f32,
  znear: f32,
  zfar: f32,

  // GPU resources owned by the camera.
  uniform: CameraUniform,
  buffer: wgpu::Buffer,
  bind_group: wgpu::BindGroup,
  bind_group_layout: wgpu::BindGroupLayout,
}

impl Camera {
  pub fn new(device: &wgpu::Device, position: Vec3, target: Vec3, aspect: f32) -> Self {
    let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
      label: Some("Camera Buffer"),
      contents: bytemuck::cast_slice(&[CameraUniform::new()]),
      usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    });

    let (bind_group, bind_group_layout) = crate::wgpu_helper::create_uniform_bindgroup(device, &buffer);

    let mut camera = Self {
      position,
      target,
      up: Vec3::Y,
      aspect,
      fovy: 45.0,
      znear: 0.1,
      zfar: 100.0,
      uniform: CameraUniform::new(),
      buffer,
      bind_group,
      bind_group_layout,
    };
    // Prime the uniform with the initial matrix. The GPU buffer is uploaded every frame by `update`.
    camera.uniform.view_proj = camera.build_view_projection_matrix().to_cols_array();

    camera
  }

  /// The bind group layout, needed when building the render pipeline.
  pub fn bind_group_layout(&self) -> &wgpu::BindGroupLayout {
    &self.bind_group_layout
  }

  /// The bind group to set on the render pass (group 1).
  pub fn bind_group(&self) -> &wgpu::BindGroup {
    &self.bind_group
  }

  /// Recompute the view-projection matrix from the current state and upload it to the GPU.
  pub fn update(&mut self, queue: &wgpu::Queue) {
    self.uniform.view_proj = self.build_view_projection_matrix().to_cols_array();
    queue.write_buffer(&self.buffer, 0, bytemuck::cast_slice(&[self.uniform]));
  }

  fn build_view_projection_matrix(&self) -> Mat4 {
    let view = glam::camera::rh::view::look_at_mat4(self.position, self.target, self.up);
    // wgpu uses the WebGPU NDC convention (Z in [0, 1], Y-up), which glam exposes as `directx`
    let proj = glam::camera::rh::proj::directx::perspective(self.fovy.to_radians(), self.aspect, self.znear, self.zfar);
    proj * view
  }

  pub fn set_position(&mut self, position: [f32; 3]) {
    self.position = Vec3::from(position);
  }

  pub fn set_aspect(&mut self, aspect: f32) {
    self.aspect = aspect;
  }
}
