// ======================================================================================
// Cubemap: a 6-layer colour texture used for IBL bakes and skybox sampling.
//
// A cubemap is a single GPU texture with `depth_or_array_layers = 6`. We expose two
// kinds of view over it:
//   - one `Cube`-dimension view for SAMPLING in shaders (`texture_cube<f32>`), and
//   - `mips * 6` single-slice 2D views used as RENDER TARGETS when baking each face.
// ======================================================================================

pub(crate) struct Cubemap {
  #[allow(dead_code)]
  pub(crate) texture: wgpu::Texture,
  // Cube-dimension view covering all 6 faces, bound for sampling.
  #[allow(dead_code)]
  pub(crate) view: wgpu::TextureView,
  // Render-target views, indexed as [mip][face]. Each is a single 2D slice.
  face_views: Vec<Vec<wgpu::TextureView>>,
  #[allow(dead_code)]
  pub(crate) sampler: wgpu::Sampler,
  #[allow(dead_code)]
  pub(crate) size: u32,
  #[allow(dead_code)]
  pub(crate) mip_level_count: u32,
  #[allow(dead_code)]
  pub(crate) label: String,
}

impl Cubemap {
  // Create an empty cubemap suitable for rendering into (RENDER_ATTACHMENT) and later
  // sampling (TEXTURE_BINDING). Faces are filled by baking render passes, not uploads.
  #[allow(dead_code)]
  pub(crate) fn new_render_target(device: &wgpu::Device, size: u32, mip_level_count: u32, format: wgpu::TextureFormat, label: &str) -> Self {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
      label: Some(label),
      size: wgpu::Extent3d {
        width: size,
        height: size,
        depth_or_array_layers: 6,
      },
      mip_level_count,
      sample_count: 1,
      dimension: wgpu::TextureDimension::D2,
      format,
      usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
      view_formats: &[],
    });

    let view = texture.create_view(&wgpu::TextureViewDescriptor {
      label: Some(&format!("{label} cube view")),
      dimension: Some(wgpu::TextureViewDimension::Cube),
      ..Default::default()
    });

    let mut face_views: Vec<Vec<wgpu::TextureView>> = Vec::with_capacity(mip_level_count as usize);
    for mip in 0..mip_level_count {
      let mut faces = Vec::with_capacity(6);
      for face in 0..6u32 {
        faces.push(texture.create_view(&wgpu::TextureViewDescriptor {
          label: Some(&format!("{label} face view mip{mip} face{face}")),
          dimension: Some(wgpu::TextureViewDimension::D2),
          base_mip_level: mip,
          mip_level_count: Some(1),
          base_array_layer: face,
          array_layer_count: Some(1),
          ..Default::default()
        }));
      }
      face_views.push(faces);
    }

    // Clamp-to-edge on all axes to avoid seams at face boundaries; linear across mips so
    // the prefiltered specular map (5c) can be sampled with `textureSampleLevel`.
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
      label: Some(&format!("{label} sampler")),
      address_mode_u: wgpu::AddressMode::ClampToEdge,
      address_mode_v: wgpu::AddressMode::ClampToEdge,
      address_mode_w: wgpu::AddressMode::ClampToEdge,
      mag_filter: wgpu::FilterMode::Linear,
      min_filter: wgpu::FilterMode::Linear,
      mipmap_filter: wgpu::MipmapFilterMode::Linear,
      ..Default::default()
    });

    Self {
      texture,
      view,
      face_views,
      sampler,
      size,
      mip_level_count,
      label: label.to_string(),
    }
  }

  // Borrow a single face's render-target view for a bake pass, addressed by mip and face.
  #[allow(dead_code)]
  pub(crate) fn face_view(&self, mip: usize, face: usize) -> &wgpu::TextureView {
    &self.face_views[mip][face]
  }
}
