use crate::engine::TextureHandle;
use crate::helpers;
use crate::models::Texture;
use crate::models::texture::texture_or_fallback;
use slotmap::SlotMap;
use wgpu::util::DeviceExt;

// material.rs
pub struct MaterialFallbacks {
  white_srgb: Texture,
  white_linear: Texture,
  flat_normal: Texture,
}

impl MaterialFallbacks {
  pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> anyhow::Result<Self> {
    let white_srgb = Texture::new_solid_color(device, queue, [255, 255, 255, 255], wgpu::TextureFormat::Rgba8UnormSrgb, "white_srgb").expect("creating default texture");
    let white_linear = Texture::new_solid_color(device, queue, [255, 255, 255, 255], wgpu::TextureFormat::Rgba8Unorm, "white_linear").expect("creating default texture");
    let flat_normal = Texture::new_solid_color(device, queue, [128, 128, 255, 255], wgpu::TextureFormat::Rgba8Unorm, "flat_normal").expect("creating default texture");

    Ok(Self {
      white_srgb,
      white_linear,
      flat_normal,
    })
  }
}

pub struct Material {
  pub base_color_factor: [f32; 4],
  pub metallic_factor: f32,
  pub roughness_factor: f32,
  pub emissive_factor: [f32; 3],

  pub alpha_mode: AlphaMode,
  pub alpha_cutoff: f32,
  pub double_sided: bool,

  pub base_color_texture: Option<TextureHandle>,
  pub metallic_roughness_texture: Option<TextureHandle>,
  pub normal_texture: Option<TextureHandle>,
  pub occlusion_texture: Option<TextureHandle>,
  pub emissive_texture: Option<TextureHandle>,

  pub normal_scale: f32,
  pub occlusion_strength: f32,

  // GPU resources
  bind_group: wgpu::BindGroup,
  uniform: MaterialUniform,
  uniform_buffer: wgpu::Buffer,
  uniform_dirty: bool,
  bind_group_dirty: bool,
}

pub enum AlphaMode {
  Opaque,
  Mask,
  Blend,
}

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct MaterialUniform {
  base_color_factor: [f32; 4],
  emissive_factor: [f32; 3],
  metallic_factor: f32,
  roughness_factor: f32,
  normal_scale: f32,
  occlusion_strength: f32,
  alpha_cutoff: f32,
}

impl Material {
  pub(crate) fn new(device: &wgpu::Device, fallbacks: &MaterialFallbacks) -> Self {
    let uniform = MaterialUniform {
      base_color_factor: [1.0, 1.0, 1.0, 1.0],
      emissive_factor: [0.0, 0.0, 0.0],
      metallic_factor: 0.0,
      roughness_factor: 0.5,
      normal_scale: 1.0,
      occlusion_strength: 1.0,
      alpha_cutoff: 0.5,
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
          resource: wgpu::BindingResource::TextureView(&fallbacks.white_srgb.view),
        },
        wgpu::BindGroupEntry {
          binding: 2,
          resource: wgpu::BindingResource::Sampler(&fallbacks.white_srgb.sampler),
        },
        wgpu::BindGroupEntry {
          binding: 3,
          resource: wgpu::BindingResource::TextureView(&fallbacks.white_linear.view),
        },
        wgpu::BindGroupEntry {
          binding: 4,
          resource: wgpu::BindingResource::Sampler(&fallbacks.white_linear.sampler),
        },
        wgpu::BindGroupEntry {
          binding: 5,
          resource: wgpu::BindingResource::TextureView(&fallbacks.flat_normal.view),
        },
        wgpu::BindGroupEntry {
          binding: 6,
          resource: wgpu::BindingResource::Sampler(&fallbacks.flat_normal.sampler),
        },
        wgpu::BindGroupEntry {
          binding: 7,
          resource: wgpu::BindingResource::TextureView(&fallbacks.white_linear.view),
        },
        wgpu::BindGroupEntry {
          binding: 8,
          resource: wgpu::BindingResource::Sampler(&fallbacks.white_linear.sampler),
        },
        wgpu::BindGroupEntry {
          binding: 9,
          resource: wgpu::BindingResource::TextureView(&fallbacks.white_srgb.view),
        },
        wgpu::BindGroupEntry {
          binding: 10,
          resource: wgpu::BindingResource::Sampler(&fallbacks.white_srgb.sampler),
        },
      ],
    });

    Material {
      base_color_factor: [1.0, 1.0, 1.0, 1.0],
      metallic_factor: 0.0,
      roughness_factor: 0.5,
      emissive_factor: [0.0, 0.0, 0.0],

      normal_scale: 1.0,
      occlusion_strength: 1.0,

      alpha_mode: AlphaMode::Opaque,
      alpha_cutoff: 0.5,
      double_sided: false,

      base_color_texture: None,
      metallic_roughness_texture: None,
      normal_texture: None,
      occlusion_texture: None,
      emissive_texture: None,

      bind_group,
      uniform,
      uniform_buffer,
      uniform_dirty: true,
      bind_group_dirty: true,
    }
  }

  pub fn set_base_color_factor(&mut self, factor: [f32; 4]) {
    self.base_color_factor = factor;
    self.uniform.base_color_factor = factor;
    self.uniform_dirty = true;
  }

  pub fn set_base_color_texture(&mut self, texture: Option<TextureHandle>) {
    self.base_color_texture = texture;
    self.bind_group_dirty = true;
  }

  pub(crate) fn get_bind_group(&self) -> &wgpu::BindGroup {
    &self.bind_group
  }

  pub(crate) fn upload_gpu(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, textures: &SlotMap<TextureHandle, Texture>, fallbacks: &MaterialFallbacks) {
    if self.uniform_dirty {
      queue.write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[self.uniform]));
      self.uniform_dirty = false;
    }

    if !self.bind_group_dirty {
      return;
    }

    let base_color = texture_or_fallback(textures, self.base_color_texture, &fallbacks.white_srgb);
    let metallic_roughness = texture_or_fallback(textures, self.metallic_roughness_texture, &fallbacks.white_linear);
    let normal = texture_or_fallback(textures, self.normal_texture, &fallbacks.flat_normal);
    let occlusion = texture_or_fallback(textures, self.occlusion_texture, &fallbacks.white_linear);
    let emissive = texture_or_fallback(textures, self.emissive_texture, &fallbacks.white_srgb);

    let layout = Self::get_bind_group_layout(device);

    self.bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
      label: Some("Material Bind Group"),
      layout: &layout,
      entries: &[
        helpers::bind_buffer(0, &self.uniform_buffer),
        helpers::bind_texture(1, base_color),
        helpers::bind_sampler(2, base_color),
        helpers::bind_texture(3, metallic_roughness),
        helpers::bind_sampler(4, metallic_roughness),
        helpers::bind_texture(5, normal),
        helpers::bind_sampler(6, normal),
        helpers::bind_texture(7, occlusion),
        helpers::bind_sampler(8, occlusion),
        helpers::bind_texture(9, emissive),
        helpers::bind_sampler(10, emissive),
      ],
    });

    self.bind_group_dirty = false;
  }

  pub(crate) fn get_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
      label: Some("Material Bind Group Layout"),
      entries: &[
        helpers::uniform_entry(0, wgpu::ShaderStages::FRAGMENT),
        helpers::texture_entry(1),
        helpers::sampler_entry(2),
        helpers::texture_entry(3),
        helpers::sampler_entry(4),
        helpers::texture_entry(5),
        helpers::sampler_entry(6),
        helpers::texture_entry(7),
        helpers::sampler_entry(8),
        helpers::texture_entry(9),
        helpers::sampler_entry(10),
      ],
    })
  }
}
