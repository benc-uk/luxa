use crate::engine::TextureHandle;
use crate::models::Texture;
use crate::{MaterialHandle, helpers};
use slotmap::SlotMap;
use wgpu::BindGroupLayout;
use wgpu::util::DeviceExt;

/// Default textures every material falls back to when a specific map is not set.
/// These live in the engine's texture arena, so materials reference them by handle
/// exactly like any user-supplied texture.
pub(crate) struct MaterialFallbacks {
  pub white_srgb: TextureHandle,
  pub white_linear: TextureHandle,
  pub flat_normal: TextureHandle,
}

impl MaterialFallbacks {
  pub(crate) fn new(device: &wgpu::Device, queue: &wgpu::Queue, textures: &mut SlotMap<TextureHandle, Texture>) -> anyhow::Result<Self> {
    let white_srgb = textures.insert(Texture::new_solid_color(
      device,
      queue,
      [255, 255, 255, 255],
      wgpu::TextureFormat::Rgba8UnormSrgb,
      "white_srgb",
    )?);

    let white_linear = textures.insert(Texture::new_solid_color(
      device,
      queue,
      [255, 255, 255, 255],
      wgpu::TextureFormat::Rgba8Unorm,
      "white_linear",
    )?);

    let flat_normal = textures.insert(Texture::new_solid_color(
      device,
      queue,
      [128, 128, 255, 255],
      wgpu::TextureFormat::Rgba8Unorm,
      "flat_normal",
    )?);

    Ok(Self {
      white_srgb,
      white_linear,
      flat_normal,
    })
  }
}

pub struct Material {
  base_color_factor: [f32; 4],
  metallic_factor: f32,
  roughness_factor: f32,
  emissive_factor: [f32; 3],

  alpha_mode: AlphaMode,
  alpha_cutoff: f32,
  double_sided: bool,

  base_color_texture: TextureHandle,
  metallic_roughness_texture: TextureHandle,
  normal_texture: TextureHandle,
  occlusion_texture: TextureHandle,
  emissive_texture: TextureHandle,

  normal_scale: f32,
  occlusion_strength: f32,

  // GPU resources
  bind_group_layout: wgpu::BindGroupLayout,
  bind_group: wgpu::BindGroup,
  uniform: MaterialUniform,
  uniform_buffer: wgpu::Buffer,
  uniform_dirty: bool,
  bind_group_dirty: bool,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Default)]
pub enum AlphaMode {
  #[default]
  Opaque,
  Mask,
  Blend,
}

/// Describes a material for `Engine::create_material`. Factors and flags mirror the glTF metallic-
/// roughness model; texture handles are optional and fall back to the engine's neutral defaults
/// when `None`.
#[derive(Debug, Clone)]
pub struct MaterialDescriptor {
  pub base_color_factor: [f32; 4],
  pub metallic_factor: f32,
  pub roughness_factor: f32,
  pub emissive_factor: [f32; 3],
  pub normal_scale: f32,
  pub occlusion_strength: f32,
  pub alpha_mode: AlphaMode,
  pub alpha_cutoff: f32,
  pub double_sided: bool,
  pub base_color_texture: Option<TextureHandle>,
  pub metallic_roughness_texture: Option<TextureHandle>,
  pub normal_texture: Option<TextureHandle>,
  pub occlusion_texture: Option<TextureHandle>,
  pub emissive_texture: Option<TextureHandle>,
}

impl Default for MaterialDescriptor {
  fn default() -> Self {
    Self {
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
    }
  }
}

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct MaterialUniform {
  base_color_factor: [f32; 4],
  emissive_factor: [f32; 3],
  metallic_factor: f32,
  roughness_factor: f32,
  normal_scale: f32,
  occlusion_strength: f32,
  alpha_cutoff: f32,
  alpha_mode: u32,    // 0 = opaque, 1 = mask, 2 = blend
  _padding: [u32; 3], // pad to 16 bytes for alignment
}

impl Material {
  pub(crate) fn new(device: &wgpu::Device, layout: &BindGroupLayout, fallbacks: &MaterialFallbacks, textures: &SlotMap<TextureHandle, Texture>) -> Self {
    let uniform = MaterialUniform {
      base_color_factor: [1.0, 1.0, 1.0, 1.0],
      emissive_factor: [0.0, 0.0, 0.0],
      metallic_factor: 0.0,
      roughness_factor: 0.5,
      normal_scale: 1.0,
      occlusion_strength: 1.0,
      alpha_cutoff: 0.5,
      alpha_mode: 0, // opaque
      _padding: [0; 3],
    };

    let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
      label: Some("Material Uniform Buffer"),
      contents: bytemuck::cast_slice(&[uniform]),
      usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    });

    // Every texture slot starts pointing at the appropriate fallback, so the material
    // is always fully bound even before the caller assigns any maps.
    let base_color_texture = fallbacks.white_srgb;
    let metallic_roughness_texture = fallbacks.white_linear;
    let normal_texture = fallbacks.flat_normal;
    let occlusion_texture = fallbacks.white_linear;
    let emissive_texture = fallbacks.white_srgb;

    let bind_group = build_bind_group(
      device,
      layout,
      &uniform_buffer,
      textures,
      base_color_texture,
      metallic_roughness_texture,
      normal_texture,
      occlusion_texture,
      emissive_texture,
    );

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

      base_color_texture,
      metallic_roughness_texture,
      normal_texture,
      occlusion_texture,
      emissive_texture,

      bind_group_layout: layout.clone(),
      bind_group,
      uniform,
      uniform_buffer,
      uniform_dirty: true,
      bind_group_dirty: false,
    }
  }

  pub fn set_base_color_factor(&mut self, factor: [f32; 4]) {
    self.base_color_factor = factor;
    self.uniform.base_color_factor = factor;
    self.uniform_dirty = true;
  }

  pub fn set_metallic_factor(&mut self, factor: f32) {
    self.metallic_factor = factor;
    self.uniform.metallic_factor = factor;
    self.uniform_dirty = true;
  }

  pub fn set_roughness_factor(&mut self, factor: f32) {
    self.roughness_factor = factor;
    self.uniform.roughness_factor = factor;
    self.uniform_dirty = true;
  }

  pub fn set_emissive_factor(&mut self, factor: [f32; 3]) {
    self.emissive_factor = factor;
    self.uniform.emissive_factor = factor;
    self.uniform_dirty = true;
  }

  pub fn set_normal_scale(&mut self, scale: f32) {
    self.normal_scale = scale;
    self.uniform.normal_scale = scale;
    self.uniform_dirty = true;
  }

  pub fn set_occlusion_strength(&mut self, strength: f32) {
    self.occlusion_strength = strength;
    self.uniform.occlusion_strength = strength;
    self.uniform_dirty = true;
  }

  pub fn set_alpha_mode(&mut self, mode: AlphaMode) {
    self.uniform.alpha_mode = match mode {
      AlphaMode::Opaque => 0,
      AlphaMode::Mask => 1,
      AlphaMode::Blend => 2,
    };

    self.alpha_mode = mode;
    self.uniform_dirty = true;
  }

  pub fn set_alpha_cutoff(&mut self, cutoff: f32) {
    self.alpha_cutoff = cutoff;
    self.uniform.alpha_cutoff = cutoff;
    self.uniform_dirty = true;
  }

  pub fn set_double_sided(&mut self, double_sided: bool) {
    self.double_sided = double_sided;
  }

  pub fn set_base_color_texture(&mut self, texture: TextureHandle) {
    self.base_color_texture = texture;
    self.bind_group_dirty = true;
  }

  pub fn set_metallic_roughness_texture(&mut self, texture: TextureHandle) {
    self.metallic_roughness_texture = texture;
    self.bind_group_dirty = true;
  }

  pub fn set_normal_texture(&mut self, texture: TextureHandle) {
    self.normal_texture = texture;
    self.bind_group_dirty = true;
  }

  pub fn set_occlusion_texture(&mut self, texture: TextureHandle) {
    self.occlusion_texture = texture;
    self.bind_group_dirty = true;
  }

  pub fn set_emissive_texture(&mut self, texture: TextureHandle) {
    self.emissive_texture = texture;
    self.bind_group_dirty = true;
  }

  pub fn is_blended(&self) -> bool {
    matches!(self.alpha_mode, AlphaMode::Blend)
  }

  pub fn is_double_sided(&self) -> bool {
    self.double_sided
  }

  pub(crate) fn get_bind_group(&self) -> &wgpu::BindGroup {
    &self.bind_group
  }

  pub(crate) fn upload_gpu(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, textures: &SlotMap<TextureHandle, Texture>) {
    if self.uniform_dirty {
      queue.write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[self.uniform]));
      self.uniform_dirty = false;
    }

    if !self.bind_group_dirty {
      return;
    }

    self.bind_group = build_bind_group(
      device,
      &self.bind_group_layout,
      &self.uniform_buffer,
      textures,
      self.base_color_texture,
      self.metallic_roughness_texture,
      self.normal_texture,
      self.occlusion_texture,
      self.emissive_texture,
    );

    self.bind_group_dirty = false;
  }
}

#[allow(clippy::too_many_arguments)]
fn build_bind_group(
  device: &wgpu::Device,
  layout: &wgpu::BindGroupLayout,
  uniform_buffer: &wgpu::Buffer,
  textures: &SlotMap<TextureHandle, Texture>,
  base_color: TextureHandle,
  metallic_roughness: TextureHandle,
  normal: TextureHandle,
  occlusion: TextureHandle,
  emissive: TextureHandle,
) -> wgpu::BindGroup {
  let get = |handle: TextureHandle| textures.get(handle).expect("material references a texture that is not in the arena");

  let base_color = get(base_color);
  let metallic_roughness = get(metallic_roughness);
  let normal = get(normal);
  let occlusion = get(occlusion);
  let emissive = get(emissive);

  device.create_bind_group(&wgpu::BindGroupDescriptor {
    label: Some("Material Bind Group"),
    layout,
    entries: &[
      helpers::bind_buffer(0, uniform_buffer),
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
  })
}

// Importing materials

pub struct ImportedMaterial {
  pub name: Option<String>,
  pub index: usize,
  pub handle: MaterialHandle,
}
