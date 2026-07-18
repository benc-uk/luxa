use crate::helpers;
use crate::models::Texture;
use wgpu::util::DeviceExt;

pub struct Material {
  base_color: [f32; 4],
  specular_color: [f32; 4],
  shininess: f32,

  // GPU resources
  bind_group: wgpu::BindGroup,
  uniform: MaterialUniform,
  uniform_buffer: wgpu::Buffer,
  dirty: bool,
}

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct MaterialUniform {
  base_color: [f32; 4],
  specular_color: [f32; 4],
  shininess: f32,
  _padding: [f32; 3],
}

impl Material {
  pub(crate) fn new(device: &wgpu::Device, texture: &Texture) -> Self {
    let base_color = [1.0, 1.0, 1.0, 1.0];
    let specular_color = [1.0, 1.0, 1.0, 1.0];
    let shininess = 32.0;

    let uniform = MaterialUniform {
      base_color,
      specular_color,
      shininess,
      _padding: [0.0; 3],
    };

    let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
      label: Some("Material Uniform Buffer"),
      contents: bytemuck::cast_slice(&[uniform]),
      usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    });

    let bind_group_layout = Self::get_bind_group_layout(device);

    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
      label: Some("Material Bind Group"),
      layout: &bind_group_layout,
      entries: &[
        wgpu::BindGroupEntry {
          binding: 0,
          resource: uniform_buffer.as_entire_binding(),
        },
        wgpu::BindGroupEntry {
          binding: 1,
          resource: wgpu::BindingResource::TextureView(&texture.view),
        },
        wgpu::BindGroupEntry {
          binding: 2,
          resource: wgpu::BindingResource::Sampler(&texture.sampler),
        },
      ],
    });

    Material {
      base_color,
      specular_color,
      shininess,
      bind_group,
      uniform,
      uniform_buffer,
      dirty: false,
    }
  }

  pub(crate) fn get_bind_group(&self) -> &wgpu::BindGroup {
    &self.bind_group
  }

  pub fn set_base_color(&mut self, color: [f32; 4]) {
    self.base_color = color;
    self.uniform.base_color = color;
    self.dirty = true;
  }

  pub fn set_specular_color(&mut self, color: [f32; 4]) {
    self.specular_color = color;
    self.uniform.specular_color = color;
    self.dirty = true;
  }

  pub fn set_shininess(&mut self, shininess: f32) {
    self.shininess = shininess;
    self.uniform.shininess = shininess;
    self.dirty = true;
  }

  pub(crate) fn upload_gpu(&mut self, queue: &wgpu::Queue) {
    if !self.dirty {
      return;
    }

    queue.write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[self.uniform]));
    self.dirty = false;
  }

  pub(crate) fn get_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
      label: Some("Material Bind Group Layout"),
      entries: &[
        helpers::uniform_entry(0, wgpu::ShaderStages::FRAGMENT),
        helpers::texture_entry(1),
        helpers::sampler_entry(2),
      ],
    })
  }
}
