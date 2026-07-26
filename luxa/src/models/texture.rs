use anyhow::Result;
use image::GenericImageView;

pub(crate) struct Texture {
  #[allow(unused)]
  pub(crate) texture: wgpu::Texture,
  pub(crate) view: wgpu::TextureView,
  pub(crate) sampler: wgpu::Sampler,
  #[allow(dead_code)]
  pub(crate) label: String,
}

impl Texture {
  #[allow(dead_code)]
  pub(crate) fn from_bytes(device: &wgpu::Device, queue: &wgpu::Queue, bytes: &[u8], format: wgpu::TextureFormat, label: &str) -> Result<Self> {
    let img = image::load_from_memory(bytes)?;
    Self::from_image(device, queue, &img, format, Some(label))
  }

  pub(crate) fn from_file(device: &wgpu::Device, queue: &wgpu::Queue, path: &str) -> Result<Self> {
    let img = image::open(path)?;
    Self::from_image(device, queue, &img, wgpu::TextureFormat::Rgba8UnormSrgb, Some(path))
  }

  pub(crate) fn from_image(device: &wgpu::Device, queue: &wgpu::Queue, img: &image::DynamicImage, format: wgpu::TextureFormat, label: Option<&str>) -> Result<Self> {
    let rgba = img.to_rgba8();
    let dimensions = img.dimensions();

    let size = wgpu::Extent3d {
      width: dimensions.0,
      height: dimensions.1,
      depth_or_array_layers: 1,
    };
    let texture = device.create_texture(&wgpu::TextureDescriptor {
      label,
      size,
      mip_level_count: 1,
      sample_count: 1,
      dimension: wgpu::TextureDimension::D2,
      format,
      usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
      view_formats: &[],
    });

    queue.write_texture(
      wgpu::TexelCopyTextureInfo {
        aspect: wgpu::TextureAspect::All,
        texture: &texture,
        mip_level: 0,
        origin: wgpu::Origin3d::ZERO,
      },
      &rgba,
      wgpu::TexelCopyBufferLayout {
        offset: 0,
        bytes_per_row: Some(4 * dimensions.0),
        rows_per_image: Some(dimensions.1),
      },
      size,
    );

    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
      address_mode_u: wgpu::AddressMode::Repeat,
      address_mode_v: wgpu::AddressMode::Repeat,
      address_mode_w: wgpu::AddressMode::Repeat,
      mag_filter: wgpu::FilterMode::Linear,
      min_filter: wgpu::FilterMode::Linear,
      mipmap_filter: wgpu::MipmapFilterMode::Linear,
      anisotropy_clamp: 16,
      ..Default::default()
    });

    Ok(Self {
      texture,
      view,
      sampler,
      label: label.unwrap_or("undefined").to_string(),
    })
  }

  // For loading equirect HDR images as a basis for IBL, this is a temporary texture sampled during the IBL baking process
  pub(crate) fn new_equirect_hdr(device: &wgpu::Device, queue: &wgpu::Queue, hdr_bytes: &[u8], label: &str) -> Result<Self> {
    let decoder = image::codecs::hdr::HdrDecoder::new(std::io::Cursor::new(hdr_bytes))?;
    let meta = decoder.metadata();
    let (w, h) = (meta.width, meta.height);
    let rgb = decoder.read_image_hdr()?; // Vec<Rgb<f32>>

    let mut texels: Vec<half::f16> = Vec::with_capacity((w * h * 4) as usize);
    for p in &rgb {
      texels.push(half::f16::from_f32(p[0]));
      texels.push(half::f16::from_f32(p[1]));
      texels.push(half::f16::from_f32(p[2]));
      texels.push(half::f16::from_f32(1.0));
    }
    // create Rgba16Float 2D texture (w,h), usage TEXTURE_BINDING | COPY_DST,
    // queue.write_texture(bytemuck::cast_slice(&texels), bytes_per_row = w*4*2),
    // build a clamp+linear sampler, return Self { .. }.

    let size = wgpu::Extent3d {
      width: w,
      height: h,
      depth_or_array_layers: 1,
    };
    let texture = device.create_texture(&wgpu::TextureDescriptor {
      label: Some(label),
      size,
      mip_level_count: 1,
      sample_count: 1,
      dimension: wgpu::TextureDimension::D2,
      format: wgpu::TextureFormat::Rgba16Float,
      usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
      view_formats: &[],
    });

    queue.write_texture(
      wgpu::TexelCopyTextureInfo {
        aspect: wgpu::TextureAspect::All,
        texture: &texture,
        mip_level: 0,
        origin: wgpu::Origin3d::ZERO,
      },
      bytemuck::cast_slice(&texels),
      wgpu::TexelCopyBufferLayout {
        offset: 0,
        bytes_per_row: Some(4 * 2 * w),
        rows_per_image: Some(h),
      },
      size,
    );

    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
      address_mode_u: wgpu::AddressMode::ClampToEdge,
      address_mode_v: wgpu::AddressMode::ClampToEdge,
      address_mode_w: wgpu::AddressMode::ClampToEdge,
      mag_filter: wgpu::FilterMode::Linear,
      min_filter: wgpu::FilterMode::Linear,
      mipmap_filter: wgpu::MipmapFilterMode::Linear,
      anisotropy_clamp: 16,
      ..Default::default()
    });

    Ok(Self {
      texture,
      view,
      sampler,
      label: label.to_string(),
    })
  }

  // A 2D texture you render INTO then sample, e.g. the BRDF integration LUT (5d).
  // No pixel upload: the content comes from a render pass, so usage is
  // RENDER_ATTACHMENT | TEXTURE_BINDING (the material path's COPY_DST isn't needed).
  pub(crate) fn new_render_target(device: &wgpu::Device, width: u32, height: u32, format: wgpu::TextureFormat, label: &str) -> Self {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
      label: Some(label),
      size: wgpu::Extent3d {
        width,
        height,
        depth_or_array_layers: 1,
      },
      mip_level_count: 1,
      sample_count: 1,
      dimension: wgpu::TextureDimension::D2,
      format,
      usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
      view_formats: &[],
    });

    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

    // Clamp so the NdotV=1 / roughness=1 edges don't wrap; linear, no mips.
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
      label: Some(&format!("{label} sampler")),
      address_mode_u: wgpu::AddressMode::ClampToEdge,
      address_mode_v: wgpu::AddressMode::ClampToEdge,
      address_mode_w: wgpu::AddressMode::ClampToEdge,
      mag_filter: wgpu::FilterMode::Linear,
      min_filter: wgpu::FilterMode::Linear,
      mipmap_filter: wgpu::MipmapFilterMode::Nearest,
      ..Default::default()
    });

    Self {
      texture,
      view,
      sampler,
      label: label.to_string(),
    }
  }

  pub(crate) fn new_solid_color(device: &wgpu::Device, queue: &wgpu::Queue, color: [u8; 4], format: wgpu::TextureFormat, label: &str) -> Result<Self> {
    let img = image::ImageBuffer::from_pixel(1, 1, image::Rgba(color));

    Self::from_image(device, queue, &image::DynamicImage::ImageRgba8(img), format, Some(label))
  }
}
